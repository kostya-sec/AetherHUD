/**
 * SAO rain + water shader layer, extracted from the accepted #7 prototype
 * (AetherHUD-gemini2, src/components/RainBackground.tsx) and adapted to the formal
 * TypeScript/React stack. Visuals are not redesigned; the only functional fixes are
 * the orthographic-camera culling/UV correction described at the shader and plane.
 */
import { useEffect, useRef } from "react";
import * as THREE from "three";

interface RainBackgroundProps {
  /**
   * 雨幕密度 0~1：
   * 0 = 特效完全关闭（不创建 WebGL 上下文）；
   * >0 时雨滴数量 = Math.round(520 * density)（至少 40 条），
   * 雨滴落水激起水面冲击波的概率 = 0.25 + 0.6 * density，
   * 冲击波 uniform 环形缓冲固定 IMPACT_MAX 个（超出覆盖最旧）。
   */
  density: number;
}

/** 水面冲击波数量上限（着色器循环次数固定，禁止动态扩容器） */
const IMPACT_MAX = 24;
/** HudBallColumn 给设置球写入的标记：水面线不得越过它。 */
const RAIN_FLOOR_SELECTOR = "[data-rain-floor]";
const WATER_VERT = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

/**
 * 真实水面片元着色器：
 * - 深水基色纵向渐变（顶部 #0d2432 → 底部 #07141d），顶部边缘细亮线模拟水天线反光；
 * - 4 组不同频率/方向正弦波 + 一层 value noise 扰动，解析梯度合成 pseudo-normal；
 * - Blinn-Phong 镜面高光（虚拟光源 normalize(vec3(-0.3, 0.8, 0.5))，pow 48）碎光；
 * - 24 个落雨冲击波（环形波峰 sin(k*(d - c*age)) * exp 衰减），扰动法线 + 波峰泡沫亮边；
 * - alpha 顶部 ~0.85 向底部渐深，冷蓝灰调与琥珀/深色 HUD 共存。
 * 坐标系：vUv 映射到容器像素（y 向下），规避 devicePixelRatio 差异。
 */
const WATER_FRAG = /* glsl */ `
  precision highp float;
  varying vec2 vUv;
  uniform float uTime;
  uniform vec2 uRes;      // 容器尺寸（CSS px）
  uniform float uWaterTop; // 水面线 y（CSS px，y 向下）
  uniform vec4 uImpacts[${IMPACT_MAX}]; // x, y(px), t0(s), amp

  float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
  }
  float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
      mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x),
      mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x),
      f.y
    );
  }

  void main() {
    // 容器像素坐标（y 向下，与 DOM 一致）。
    // 正交相机使用 top=0/bottom=height，平面局部 +Y 投影到屏幕下方，
    // 因此 vUv.y 本身即屏幕 y 方向；此前的 (1.0 - vUv.y) 会把水面画到顶部。
    vec2 p = vec2(vUv.x * uRes.x, vUv.y * uRes.y);
    if (p.y < uWaterTop) discard;
    float bandH = max(uRes.y - uWaterTop, 1.0);
    float depth = clamp((p.y - uWaterTop) / bandH, 0.0, 1.0);

    float t = uTime;

    // ---- 程序化波纹 pseudo-normal：4 正弦波解析梯度 ----
    vec2 g = vec2(0.0);
    {
      vec2 k1 = vec2(0.10, 0.24);
      vec2 k2 = vec2(-0.07, 0.18);
      vec2 k3 = vec2(0.21, -0.06);
      vec2 k4 = vec2(-0.16, -0.12);
      g += 1.15 * k1 * cos(dot(k1, p) + t * 1.6);
      g += 0.85 * k2 * cos(dot(k2, p) + t * 1.1 + 1.7);
      g += 0.50 * k3 * cos(dot(k3, p) + t * 2.3 + 3.9);
      g += 0.32 * k4 * cos(dot(k4, p) - t * 2.9 + 0.6);
    }
    // hash noise 扰动（value noise 中心差分梯度）
    {
      vec2 np = p * 0.16 + vec2(t * 0.35, -t * 0.22);
      float e = 0.9;
      float nx = vnoise(np + vec2(e, 0.0)) - vnoise(np - vec2(e, 0.0));
      float ny = vnoise(np + vec2(0.0, e)) - vnoise(np - vec2(0.0, e));
      g += vec2(nx, ny) * 0.9;
    }

    // ---- 落雨冲击波：环形波峰扩散，扰动法线 + 波峰泡沫 ----
    float foam = 0.0;
    for (int i = 0; i < ${IMPACT_MAX}; i++) {
      vec4 imp = uImpacts[i];
      float amp = imp.w;
      if (amp < 0.001) continue;
      float age = t - imp.z;
      if (age < 0.0 || age > 1.2) continue;
      vec2 dv = p - imp.xy;
      float d = length(dv);
      float r = age * 115.0; // 波前扩散速度 px/s
      float envelope = exp(-abs(d - r) * 0.10) * exp(-age * 2.6) * amp;
      float slope = cos((d - r) * 0.45) * envelope;
      g += (dv / max(d, 0.001)) * slope * 5.0;
      foam += max(sin((d - r) * 0.45), 0.0) * envelope;
    }

    // 俯视水面：纵向梯度压缩，模拟浅视角透视
    vec3 n = normalize(vec3(-g.x * 0.55, -g.y * 0.30, 1.0));

    // ---- 深水基色：顶部 #0d2432 → 底部 #07141d ----
    vec3 deepTop = vec3(0.051, 0.141, 0.196);
    vec3 deepBot = vec3(0.027, 0.078, 0.114);
    vec3 col = mix(deepTop, deepBot, depth);
    // 顶部边缘细亮线：水天线反光
    float edge = exp(-depth * 22.0);
    col += vec3(0.10, 0.17, 0.19) * edge;

    // ---- Blinn-Phong 镜面高光：虚拟光源碎光 ----
    vec3 L = normalize(vec3(-0.3, 0.8, 0.5));
    vec3 H = normalize(L + vec3(0.0, 0.0, 1.0));
    float ndh = max(dot(n, H), 0.0);
    float spec = pow(ndh, 48.0) * 0.42 + pow(ndh, 96.0) * 0.20;
    col += vec3(0.85, 0.94, 1.0) * spec;

    // 冲击波峰泡沫亮边（克制的冷白，不抢戏）
    col += vec3(0.45, 0.62, 0.68) * foam * 0.55;

    // ---- 透明度：顶部 ~0.85 向底部渐深，高光/泡沫处略实 ----
    float alpha = mix(0.85, 0.96, depth) + edge * 0.05 + spec * 0.35 + foam * 0.25;
    gl_FragColor = vec4(col, clamp(alpha, 0.0, 1.0));
  }
`;

/**
 * SAO 雨幕 + 着色器水面特效（three.js，真实水面升级版）。
 * 渲染范围严格限定在父容器（absolute inset-0，父级需 relative）。
 * - 雨滴：单条 LineSegments，vertexColors 实现「亮头 + 渐隐尾」运动拖尾；
 *   每滴雨的速度 / 长度 / 亮度 / 风摆相位均有随机差异，全局有缓慢正弦风摆。
 * - 水面：waterLine 到容器底部一块全宽单位 Plane + 自定义 ShaderMaterial
 *   （深水渐变 + 程序化波纹法线 + Blinn-Phong 碎光 + 24 路落雨冲击波），
 *   一个 draw call；着色器编译失败时静默隐藏水面（console.warn），不白屏。
 * - 水花：落水瞬间抛起 3~5 个亮点（单个 THREE.Points 池化），生命周期 < 0.3s。
 * 水面线（waterLine）每 ~15 帧动态计算：若 HUD 内存在带 data-rain-floor 的
 * 「设置球」元素，水面下限不低于该元素底部；否则回退容器高度 88%。
 * 总 draw call = 3（水面 1 + 雨滴 1 + 水花 1）。
 */
export function RainBackground({ density }: RainBackgroundProps) {
  const mountRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = mountRef.current;
    if (!container || density <= 0) return;

    let width = container.clientWidth || 800;
    let height = container.clientHeight || 400;

    // 正交相机：以像素为单位，方便映射到容器坐标
    const scene = new THREE.Scene();
    const camera = new THREE.OrthographicCamera(0, width, 0, height, 0.1, 100);
    camera.position.z = 10;

    const renderer = new THREE.WebGLRenderer({
      alpha: true,
      antialias: true,
      powerPreference: 'low-power'
    });
    renderer.setSize(width, height);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    // ---- 密度驱动的动态参数 ----
    const densityValue = Math.min(1, Math.max(0, density));
    const DROP_COUNT = Math.max(40, Math.round(520 * densityValue));
    const IMPACT_PROB = 0.25 + 0.6 * densityValue; // 雨滴落水激起水面冲击波的概率
    const SPLASH_PROB = Math.min(1, IMPACT_PROB + 0.25); // 溅起水花的概率
    const SPLASH_POOL = Math.max(24, Math.round(160 * densityValue)); // 水花粒子池

    // 水面线：默认容器底部带状区，动态受 data-rain-floor 地板约束
    let waterLine = height * 0.88;

    // ---- 着色器水面：单位 Plane 重铺满屏，片元内按 uWaterTop 裁剪水面带 ----
    const impactData: THREE.Vector4[] = [];
    for (let i = 0; i < IMPACT_MAX; i++) impactData.push(new THREE.Vector4(0, 0, -10, 0));
    let impactCursor = 0;
    const waterUniforms = {
      uTime: { value: 0 },
      uRes: { value: new THREE.Vector2(width, height) },
      uWaterTop: { value: waterLine },
      uImpacts: { value: impactData }
    };
    let water: THREE.Mesh | null = null;
    let waterMat: THREE.ShaderMaterial | null = null;
    let waterGeo: THREE.PlaneGeometry | null = null;
    let waterBroken = false;
    const fitWater = () => {
      if (!water) return;
      water.scale.set(width, height, 1);
      water.position.set(width / 2, height / 2, 0);
    };
    try {
      waterGeo = new THREE.PlaneGeometry(1, 1);
      waterMat = new THREE.ShaderMaterial({
        vertexShader: WATER_VERT,
        fragmentShader: WATER_FRAG,
        uniforms: waterUniforms,
        transparent: true,
        depthWrite: false,
        blending: THREE.NormalBlending,
        // The orthographic camera uses top=0/bottom=height (screen-space y down),
        // which inverts face winding; the unit plane would otherwise be back-face culled.
        side: THREE.DoubleSide
      });
      water = new THREE.Mesh(waterGeo, waterMat);
      water.frustumCulled = false;
      water.renderOrder = 0;
      fitWater();
      scene.add(water);
      // 着色器编译失败静默回退：隐藏水面带，保留雨滴/水花，不白屏
      renderer.debug.onShaderError = (gl, program, vs, fs) => {
        waterBroken = true;
        console.warn(
          '[RainBackground] 水面着色器编译失败，已静默隐藏水面：',
          gl.getProgramInfoLog(program),
          gl.getShaderInfoLog(vs),
          gl.getShaderInfoLog(fs)
        );
        if (water) water.visible = false;
      };
      // 强制同步编译一次，尽早触发 onShaderError
      renderer.compile(scene, camera);

    } catch (err) {
      waterBroken = true;
      console.warn('[RainBackground] 水面初始化失败，已静默跳过：', err);
      if (water) water.visible = false;
    }

    // ---- 雨滴：LineSegments + vertexColors「亮头渐隐尾」 ----
    // 加色混合下，尾部顶点色接近黑色即自然渐隐，无需逐顶点 alpha
    const dropPositions = new Float32Array(DROP_COUNT * 6);
    const dropColors = new Float32Array(DROP_COUNT * 6);
    const dropSpeed = new Float32Array(DROP_COUNT);
    const dropLen = new Float32Array(DROP_COUNT);
    const dropPhase = new Float32Array(DROP_COUNT); // 风摆相位差
    const HEAD_RGB = [0.78, 0.94, 1.0]; // 亮头偏白青（比原 0xa5ecff 略亮）
    for (let i = 0; i < DROP_COUNT; i++) {
      const x = Math.random() * width;
      const y = Math.random() * height;
      dropLen[i] = 16 + Math.random() * 36;
      dropSpeed[i] = 8 + Math.random() * 12;
      dropPhase[i] = Math.random() * Math.PI * 2;
      const bright = 0.55 + Math.random() * 0.45; // 每滴亮度差异
      dropPositions.set([x, y, 0, x + 1.2, y - dropLen[i], 0], i * 6);
      dropColors.set(
        [
          HEAD_RGB[0] * bright,
          HEAD_RGB[1] * bright,
          HEAD_RGB[2] * bright,
          HEAD_RGB[0] * bright * 0.04,
          HEAD_RGB[1] * bright * 0.04,
          HEAD_RGB[2] * bright * 0.04
        ],
        i * 6
      );
    }
    const dropGeo = new THREE.BufferGeometry();
    dropGeo.setAttribute('position', new THREE.BufferAttribute(dropPositions, 3));
    dropGeo.setAttribute('color', new THREE.BufferAttribute(dropColors, 3));
    const dropMat = new THREE.LineBasicMaterial({
      vertexColors: true,
      transparent: true,
      opacity: 0.9,
      blending: THREE.AdditiveBlending,
      depthWrite: false
    });
    const rain = new THREE.LineSegments(dropGeo, dropMat);
    rain.frustumCulled = false; // 顶点每帧重写，跳过包围球剔除
    rain.renderOrder = 1;
    scene.add(rain);

    // ---- 水花：单个 THREE.Points 池化，落水瞬间 3~5 个亮点短促抛起 ----
    const splashPositions = new Float32Array(SPLASH_POOL * 3).fill(-9999);
    const splashColors = new Float32Array(SPLASH_POOL * 3);
    const splashVelX = new Float32Array(SPLASH_POOL);
    const splashVelY = new Float32Array(SPLASH_POOL);
    const splashAge = new Float32Array(SPLASH_POOL).fill(1);
    const splashLife = new Float32Array(SPLASH_POOL).fill(1);
    const splashGeo = new THREE.BufferGeometry();
    splashGeo.setAttribute('position', new THREE.BufferAttribute(splashPositions, 3));
    splashGeo.setAttribute('color', new THREE.BufferAttribute(splashColors, 3));
    const splashMat = new THREE.PointsMaterial({
      size: 3,
      sizeAttenuation: false, // 正交相机下固定像素尺寸
      vertexColors: true,
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false
    });
    const splash = new THREE.Points(splashGeo, splashMat);
    splash.frustumCulled = false;
    splash.renderOrder = 2;
    scene.add(splash);
    let splashCursor = 0;
    const spawnSplash = (x: number, y: number) => {
      const n = 3 + Math.floor(Math.random() * 3); // 3~5 个亮点
      for (let k = 0; k < n; k++) {
        const i = splashCursor;
        splashCursor = (splashCursor + 1) % SPLASH_POOL;
        splashPositions[i * 3] = x + (Math.random() - 0.5) * 4;
        splashPositions[i * 3 + 1] = y;
        splashPositions[i * 3 + 2] = 0;
        splashVelX[i] = (Math.random() - 0.5) * 140; // px/s
        splashVelY[i] = -(90 + Math.random() * 160); // 向上抛起（y 向下为正）
        splashAge[i] = 0;
        splashLife[i] = 0.15 + Math.random() * 0.13; // 生命周期 < 0.3s
      }
    };

    // 尺寸自适应
    const resizeObserver = new ResizeObserver(() => {
      width = container.clientWidth || width;
      height = container.clientHeight || height;
      camera.right = width;
      camera.bottom = height;
      camera.updateProjectionMatrix();
      renderer.setSize(width, height);
      waterUniforms.uRes.value.set(width, height);
      fitWater();
    });
    resizeObserver.observe(container);

    // 水面线动态计算：水面下限不低于「设置球」（data-rain-floor）底部。
    // 节流执行（每 ~15 帧查一次 DOM，避免每帧强制布局）。
    let frameCount = 0;
    const updateWaterLine = () => {
      const floorEl =
        container.querySelector(RAIN_FLOOR_SELECTOR) ??
        container.parentElement?.querySelector(RAIN_FLOOR_SELECTOR) ??
        null;
      let floorY = NaN;
      if (floorEl) {
        const floorRect = floorEl.getBoundingClientRect();
        const containerRect = container.getBoundingClientRect();
        const y = floorRect.bottom - containerRect.top;
        if (Number.isFinite(y) && y > 0) floorY = y;
      }
      waterLine = Number.isFinite(floorY) ? Math.min(height * 0.88, floorY) : height * 0.88;
      waterUniforms.uWaterTop.value = waterLine;
    };
    updateWaterLine();

    let rafId = 0;
    let time = 0;
    let lastTime = performance.now();

    // 冲击波环形缓冲：雨滴落水时写入 (x, y, t0, amp)，超出 24 个覆盖最旧
    const addImpact = (x: number) => {
      const bandH = Math.max(height - waterLine, 1);
      const y = waterLine + 2 + Math.random() * bandH * 0.55; // 略低于水面线，带纵深感
      const amp = (0.45 + Math.random() * 0.6) * (0.7 + 0.3 * densityValue);
      const v = impactData[impactCursor];
      impactCursor = (impactCursor + 1) % IMPACT_MAX;
      v.set(x, y, time, amp);
    };

    const animate = () => {
      rafId = requestAnimationFrame(animate);
      if (++frameCount % 15 === 0) updateWaterLine();

      const now = performance.now();
      const dt = Math.min(0.05, (now - lastTime) / 1000); // 秒， clamp 防后台切回跳变
      lastTime = now;
      time += dt;
      waterUniforms.uTime.value = time;

      // 全局风摆：缓慢正弦漂移
      const windGlobal = Math.sin(time * 0.55) * 0.5;

      // 雨滴
      const pos = dropGeo.attributes.position.array as Float32Array;
      for (let i = 0; i < DROP_COUNT; i++) {
        const base = i * 6;
        let y = pos[base + 1] + dropSpeed[i];
        // 基础斜落 + 全局风摆 + 每滴相位差的微小抖动
        let x = pos[base] - 0.35 + windGlobal + Math.sin(time * 1.1 + dropPhase[i]) * 0.18;
        if (y >= waterLine) {
          const r = Math.random();
          if (r < IMPACT_PROB && !waterBroken) addImpact(x);
          if (r < SPLASH_PROB) spawnSplash(x, waterLine);
          x = Math.random() * width;
          y = -dropLen[i] - Math.random() * 40;
        }
        // 雨尾随风轻微倾斜
        const tilt = 1.2 + windGlobal * dropLen[i] * 0.06;
        pos[base] = x;
        pos[base + 1] = y;
        pos[base + 3] = x + tilt;
        pos[base + 4] = y - dropLen[i];
      }
      dropGeo.attributes.position.needsUpdate = true;

      // 水花粒子：重力抛物线 + 颜色渐隐
      const GRAV = 900; // px/s^2
      let splashDirty = false;
      for (let i = 0; i < SPLASH_POOL; i++) {
        if (splashAge[i] >= splashLife[i]) continue;
        splashDirty = true;
        splashAge[i] += dt;
        const t = splashAge[i] / splashLife[i];
        if (t >= 1) {
          splashPositions[i * 3 + 1] = -9999; // 移出画面
          splashColors[i * 3] = 0;
          splashColors[i * 3 + 1] = 0;
          splashColors[i * 3 + 2] = 0;
          continue;
        }
        splashVelY[i] += GRAV * dt;
        splashPositions[i * 3] += splashVelX[i] * dt;
        splashPositions[i * 3 + 1] += splashVelY[i] * dt;
        const fade = Math.pow(1 - t, 1.2) * 0.9;
        splashColors[i * 3] = 0.75 * fade;
        splashColors[i * 3 + 1] = 0.95 * fade;
        splashColors[i * 3 + 2] = 1.0 * fade;
      }
      if (splashDirty) {
        splashGeo.attributes.position.needsUpdate = true;
        splashGeo.attributes.color.needsUpdate = true;
      }

      renderer.render(scene, camera);
    };
    animate();

    return () => {
      cancelAnimationFrame(rafId);
      resizeObserver.disconnect();
      if (renderer.domElement.parentElement === container) {
        container.removeChild(renderer.domElement);
      }
      dropGeo.dispose();
      dropMat.dispose();
      waterGeo?.dispose();
      waterMat?.dispose();
      splashGeo.dispose();
      splashMat.dispose();
      renderer.dispose();
    };
    // 密度变化时重建整个场景（Float32Array 尺寸随之变化）
  }, [density]);

  if (density <= 0) return null;

  return (
    <div
      ref={mountRef}
      className="absolute inset-0 pointer-events-none overflow-hidden rounded-2xl"
      style={{ zIndex: 0 }}
      aria-hidden="true"
    />
  );
}
