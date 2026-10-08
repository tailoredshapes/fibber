// WGSL of the kernels of a fibber program, written by fibc (docs/design/webgpu.md): group 0 is binding 0 the uniform of scalar
// parameters (4 bytes each, in order), binding 1 the trap flag, bindings 2.. the pointer parameters in order; wg_x/wg_y/wg_z the workgroup size.
// fib.kernel-sig vadd: ptr ptr ptr i32
// fib.kernel-sig vaddi: ptr ptr ptr i32
// fib.kernel-sig gemm: ptr ptr ptr i32
// fib.kernel-sig gemm_smem: ptr ptr ptr i32
// fib.kernel-sig assert_positive: ptr i32
struct Ptr { buf: u32, off: u32 }
struct OvI32 { v: i32, o: bool }
struct FibTrap { flag: atomic<u32> }
override wg_x: u32 = 64u;
override wg_y: u32 = 1u;
override wg_z: u32 = 1u;
@group(0) @binding(0) var<uniform> fibw_params: array<vec4<u32>, 4>;
@group(0) @binding(1) var<storage, read_write> fibw_flag: FibTrap;
@group(0) @binding(2) var<storage, read_write> buf0: array<u32>;
@group(0) @binding(3) var<storage, read_write> buf1: array<u32>;
@group(0) @binding(4) var<storage, read_write> buf2: array<u32>;
var<private> fibw_lid: vec3<u32>;
var<private> fibw_wid: vec3<u32>;
var<private> fibw_nwg: vec3<u32>;
var<private> fibw_trapped: bool = false;
fn fibw_trap() { fibw_trapped = true; atomicStore(&fibw_flag.flag, 1u); }
var<workgroup> fibw_sh0: array<u32, 1024>;
var<workgroup> fibw_sh1: array<u32, 1024>;
fn fibw_ld_u32(p: Ptr) -> u32 { switch p.buf { case 0u: { return buf0[p.off >> 2u]; } case 1u: { return buf1[p.off >> 2u]; } case 2u: { return buf2[p.off >> 2u]; } case 16u: { return fibw_sh0[p.off >> 2u]; } case 17u: { return fibw_sh1[p.off >> 2u]; } default: { return 0u; } } }
fn fibw_st_u32(p: Ptr, v: u32) { switch p.buf { case 0u: { buf0[p.off >> 2u] = v; } case 1u: { buf1[p.off >> 2u] = v; } case 2u: { buf2[p.off >> 2u] = v; } case 16u: { fibw_sh0[p.off >> 2u] = v; } case 17u: { fibw_sh1[p.off >> 2u] = v; } default: { } } }
fn fibw_ld_f32(p: Ptr) -> f32 { return bitcast<f32>(fibw_ld_u32(p)); }
fn fibw_ld_i32(p: Ptr) -> i32 { return bitcast<i32>(fibw_ld_u32(p)); }
fn fibw_st_f32(p: Ptr, v: f32) { fibw_st_u32(p, bitcast<u32>(v)); }
fn fibw_st_i32(p: Ptr, v: i32) { fibw_st_u32(p, bitcast<u32>(v)); }
fn fibw_sadd_ovf(a: i32, b: i32) -> OvI32 { let r = bitcast<i32>(bitcast<u32>(a) + bitcast<u32>(b)); return OvI32(r, ((a ^ r) & (b ^ r)) < i32(0)); }
fn fibw_ssub_ovf(a: i32, b: i32) -> OvI32 { let r = bitcast<i32>(bitcast<u32>(a) - bitcast<u32>(b)); return OvI32(r, ((a ^ b) & (a ^ r)) < i32(0)); }
fn fibw_smul_ovf(a: i32, b: i32) -> OvI32 { let r = bitcast<i32>(bitcast<u32>(a) * bitcast<u32>(b)); let o = (a != i32(0)) && ((r / a != b) || ((a == i32(-1)) && (b == i32(-2147483648)))); return OvI32(r, o); }
fn fib_trap_c(a_a0: Ptr) {
  var a0: Ptr = a_a0;
    fibw_trap();
    return;
}
fn fib_trap(a_a0: Ptr) {
  var a0: Ptr = a_a0;
    fibw_trap();
    return;
}
fn fibw_k_vadd(a_p0: Ptr, a_p1: Ptr, a_p2: Ptr, a_p3: i32) {
  var p0: Ptr = a_p0;
  var p1: Ptr = a_p1;
  var p2: Ptr = a_p2;
  var p3: i32 = a_p3;
  var t8: f32;
  var t9: f32;
  var t10: f32;
  var t4: bool;
  var t1: i32;
    t1 = f_gpu32_global_id_x(); 
    t4 = (t1 < p3); if t4 { 
    t8 = f_gpu32_f32_at(p0, t1); t9 = f_gpu32_f32_at(p1, t1); t10 = (t8 + t9); f_gpu32_f32_set_(p2, t1, t10);
    
 } else { 
    
 }
    return;


}
@compute @workgroup_size(wg_x, wg_y, wg_z)
fn vadd(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
  fibw_lid = lid; fibw_wid = wid; fibw_nwg = nwg;
  fibw_k_vadd(Ptr(0u, 0u), Ptr(1u, 0u), Ptr(2u, 0u), bitcast<i32>(fibw_params[0].x));
}
fn fibw_k_vaddi(a_p0: Ptr, a_p1: Ptr, a_p2: Ptr, a_p3: i32) {
  var p0: Ptr = a_p0;
  var p1: Ptr = a_p1;
  var p2: Ptr = a_p2;
  var p3: i32 = a_p3;
  var t8: i32;
  var t11: bool;
  var t14: i32;
  var t9: i32;
  var t10: OvI32;
  var t4: bool;
  var t1: i32;
    t1 = f_gpu32_global_id_x(); 
    t4 = (t1 < p3); if t4 { 
    t8 = f_gpu32_i32_at(p0, t1); t9 = f_gpu32_i32_at(p1, t1); t10 = fibw_sadd_ovf(t8, t9); t11 = t10.o; if t11 { 
    fib_trap_c(Ptr(0u, 0u)); if (fibw_trapped) { return; }
    return;
 } else {  }
    t14 = t10.v; f_gpu32_i32_set_(p2, t1, t14);
    

 } else { 
    
 }
    return;


}
@compute @workgroup_size(wg_x, wg_y, wg_z)
fn vaddi(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
  fibw_lid = lid; fibw_wid = wid; fibw_nwg = nwg;
  fibw_k_vaddi(Ptr(0u, 0u), Ptr(1u, 0u), Ptr(2u, 0u), bitcast<i32>(fibw_params[0].x));
}
fn fibw_k_gemm(a_p0: Ptr, a_p1: Ptr, a_p2: Ptr, a_p3: i32) {
  var p0: Ptr = a_p0;
  var p1: Ptr = a_p1;
  var p2: Ptr = a_p2;
  var p3: i32 = a_p3;
  var t102: f32;
  var t45: f32;
  var t30: i32;
  var t159: i32;
  var t95: f32;
  var t127: i32;
  var t113: f32;
  var t5: i32;
  var t166: i32;
  var t69: i32;
  var t140: f32;
  var t54: i32;
  var t98: f32;
  var t114: f32;
  var t2: i32;
  var t80: f32;
  var t161: f32;
  var t147: f32;
  var t53: i32;
  var t155: i32;
  var t70: f32;
  var t49: bool;
  var t105: f32;
  var t42: f32;
  var t37: f32;
  var t100: f32;
  var t32: f32;
  var t14: bool;
  var t118: f32;
  var t121: f32;
  var t62: f32;
  var t93: i32;
  var t89: i32;
  var t124: f32;
  var t96: f32;
  var t163: f32;
  var t145: i32;
  var t116: f32;
  var t18: i32;
  var t131: i32;
  var t157: i32;
  var t39: f32;
  var t150: i32;
  var t75: f32;
  var t106: f32;
  var t41: f32;
  var t34: f32;
  var t78: i32;
  var t123: f32;
  var t149: f32;
  var t126: f32;
  var t65: i32;
  var t58: f32;
  var t94: i32;
  var t9: bool;
  var t165: f32;
  var t143: i32;
  var t110: f32;
  var t6: i32;
  var t84: i32;
  var t146: f32;
  var t115: f32;
  var t160: f32;
  var t109: f32;
  var t152: i32;
  var t134: i32;
  var t104: f32;
  var t43: f32;
  var t36: f32;
  var t31: f32;
  var t158: f32;
  var t103: f32;
  var t44: f32;
  var t120: f32;
  var t119: f32;
  var t141: i32;
  var t55: i32;
  var t99: f32;
  var t112: f32;
  var t167: f32;
  var t24: i32;
  var t144: f32;
  var t117: f32;
  var t1: i32;
  var t83: i32;
  var t162: i32;
  var t21: i32;
  var t154: f32;
  var t48: i32;
  var t38: f32;
  var t151: f32;
  var t74: i32;
  var t137: i32;
  var t33: f32;
  var t101: f32;
  var t46: f32;
  var t148: i32;
  var t90: f32;
  var t122: f32;
  var t61: i32;
  var t97: f32;
  var t88: i32;
  var t125: f32;
  var t66: f32;
  var t111: f32;
  var t85: f32;
  var t164: i32;
  var t27: i32;
  var t128: i32;
  var t142: f32;
  var t156: f32;
  var t73: i32;
  var t153: f32;
  var t108: f32;
  var t79: i32;
  var t13: bool;
  var t107: f32;
  var t40: f32;
  var t35: f32;
  var ph115964117771: bool;
                     t1 = f_gpu32_global_id_x(); t2 = (i32(4) * t1); 
    t5 = f_gpu32_global_id_y(); t6 = (i32(4) * t5); 
    t9 = (t6 < p3); if t9 { 
    t13 = (t2 < p3); ph115964117771 = t13; 
 } else { 
    ph115964117771 = false; 
 }
    t14 = ph115964117771; if t14 { 
    t18 = (t6 * p3); 
    t21 = (t18 + p3); 
    t24 = (t21 + p3); 
    t27 = (t24 + p3); 
    t30 = i32(0);
    t31 = f32(0.0);
    t32 = f32(0.0);
    t33 = f32(0.0);
    t34 = f32(0.0);
    t35 = f32(0.0);
    t36 = f32(0.0);
    t37 = f32(0.0);
    t38 = f32(0.0);
    t39 = f32(0.0);
    t40 = f32(0.0);
    t41 = f32(0.0);
    t42 = f32(0.0);
    t43 = f32(0.0);
    t44 = f32(0.0);
    t45 = f32(0.0);
    t46 = f32(0.0);
    loop {
    t48 = t30; t49 = (t48 < p3); if t49 { 
    t53 = t30; t54 = (t53 * p3); t55 = (t54 + t2); 
    t58 = f_gpu32_f32_at(p1, t55); 
    t61 = (t55 + i32(1)); t62 = f_gpu32_f32_at(p1, t61); 
    t65 = (t55 + i32(2)); t66 = f_gpu32_f32_at(p1, t65); 
    t69 = (t55 + i32(3)); t70 = f_gpu32_f32_at(p1, t69); 
    t73 = t30; t74 = (t18 + t73); t75 = f_gpu32_f32_at(p0, t74); 
    t78 = t30; t79 = (t21 + t78); t80 = f_gpu32_f32_at(p0, t79); 
    t83 = t30; t84 = (t24 + t83); t85 = f_gpu32_f32_at(p0, t84); 
    t88 = t30; t89 = (t27 + t88); t90 = f_gpu32_f32_at(p0, t89); 
    t93 = t30; t94 = (t93 + i32(1)); t95 = t31; t96 = fma(t75, t58, t95); t97 = t32; t98 = fma(t75, t62, t97); t99 = t33; t100 = fma(t75, t66, t99); t101 = t34; t102 = fma(t75, t70, t101); t103 = t35; t104 = fma(t80, t58, t103); t105 = t36; t106 = fma(t80, t62, t105); t107 = t37; t108 = fma(t80, t66, t107); t109 = t38; t110 = fma(t80, t70, t109); t111 = t39; t112 = fma(t85, t58, t111); t113 = t40; t114 = fma(t85, t62, t113); t115 = t41; t116 = fma(t85, t66, t115); t117 = t42; t118 = fma(t85, t70, t117); t119 = t43; t120 = fma(t90, t58, t119); t121 = t44; t122 = fma(t90, t62, t121); t123 = t45; t124 = fma(t90, t66, t123); t125 = t46; t126 = fma(t90, t70, t125); t30 = t94;
    t31 = t96;
    t32 = t98;
    t33 = t100;
    t34 = t102;
    t35 = t104;
    t36 = t106;
    t37 = t108;
    t38 = t110;
    t39 = t112;
    t40 = t114;
    t41 = t116;
    t42 = t118;
    t43 = t120;
    t44 = t122;
    t45 = t124;
    t46 = t126;
    continue;









 } else { break; }
}

    t127 = (t6 * p3); t128 = (t127 + t2); 
    t131 = (t128 + p3); 
    t134 = (t131 + p3); 
    t137 = (t134 + p3); 
    t140 = t31; f_gpu32_f32_set_(p2, t128, t140);
    t141 = (t128 + i32(1)); t142 = t32; f_gpu32_f32_set_(p2, t141, t142);
    t143 = (t128 + i32(2)); t144 = t33; f_gpu32_f32_set_(p2, t143, t144);
    t145 = (t128 + i32(3)); t146 = t34; f_gpu32_f32_set_(p2, t145, t146);
    t147 = t35; f_gpu32_f32_set_(p2, t131, t147);
    t148 = (t131 + i32(1)); t149 = t36; f_gpu32_f32_set_(p2, t148, t149);
    t150 = (t131 + i32(2)); t151 = t37; f_gpu32_f32_set_(p2, t150, t151);
    t152 = (t131 + i32(3)); t153 = t38; f_gpu32_f32_set_(p2, t152, t153);
    t154 = t39; f_gpu32_f32_set_(p2, t134, t154);
    t155 = (t134 + i32(1)); t156 = t40; f_gpu32_f32_set_(p2, t155, t156);
    t157 = (t134 + i32(2)); t158 = t41; f_gpu32_f32_set_(p2, t157, t158);
    t159 = (t134 + i32(3)); t160 = t42; f_gpu32_f32_set_(p2, t159, t160);
    t161 = t43; f_gpu32_f32_set_(p2, t137, t161);
    t162 = (t137 + i32(1)); t163 = t44; f_gpu32_f32_set_(p2, t162, t163);
    t164 = (t137 + i32(2)); t165 = t45; f_gpu32_f32_set_(p2, t164, t165);
    t166 = (t137 + i32(3)); t167 = t46; f_gpu32_f32_set_(p2, t166, t167);
    
    










 } else { 
    
 }
    return;




}
@compute @workgroup_size(wg_x, wg_y, wg_z)
fn gemm(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
  fibw_lid = lid; fibw_wid = wid; fibw_nwg = nwg;
  fibw_k_gemm(Ptr(0u, 0u), Ptr(1u, 0u), Ptr(2u, 0u), bitcast<i32>(fibw_params[0].x));
}
fn fibw_k_gemm_smem(a_p0: Ptr, a_p1: Ptr, a_p2: Ptr, a_p3: i32) {
  var p0: Ptr = a_p0;
  var p1: Ptr = a_p1;
  var p2: Ptr = a_p2;
  var p3: i32 = a_p3;
  var t173: f32;
  var t245: f32;
  var t102: f32;
  var t230: f32;
  var t30: i32;
  var t8: i32;
  var t5: i32;
  var t225: f32;
  var t166: f32;
  var t25: bool;
  var t269: i32;
  var t254: f32;
  var t54: f32;
  var t298: f32;
  var t98: f32;
  var t114: i32;
  var t280: f32;
  var t161: i32;
  var t22: bool;
  var t147: f32;
  var t191: f32;
  var t70: f32;
  var t249: f32;
  var t270: i32;
  var t242: f32;
  var t42: i32;
  var t237: f32;
  var t37: bool;
  var t47: i32;
  var t171: f32;
  var t14: bool;
  var t121: i32;
  var t62: f32;
  var t182: f32;
  var t267: i32;
  var t124: i32;
  var t201: f32;
  var t296: f32;
  var t82: f32;
  var t189: f32;
  var t251: f32;
  var t116: i32;
  var t282: f32;
  var t18: i32;
  var t131: i32;
  var t272: f32;
  var t157: i32;
  var t218: f32;
  var t239: f32;
  var t39: i32;
  var t194: f32;
  var t150: i32;
  var t136: i32;
  var t106: f32;
  var t34: i32;
  var t278: f32;
  var t78: f32;
  var t212: f32;
  var t177: f32;
  var t12: bool;
  var t185: f32;
  var t260: f32;
  var t206: f32;
  var t265: i32;
  var t203: f32;
  var t58: f32;
  var t294: f32;
  var t94: f32;
  var t9: bool;
  var t165: i32;
  var t26: bool;
  var t129: i32;
  var t143: f32;
  var t257: f32;
  var t110: f32;
  var t284: f32;
  var t300: f32;
  var t209: f32;
  var t146: i32;
  var t134: i32;
  var t43: i32;
  var t236: f32;
  var t36: i32;
  var t139: f32;
  var t243: f32;
  var t231: f32;
  var t31: i32;
  var t158: f32;
  var t17: i32;
  var t263: f32;
  var t292: f32;
  var t255: f32;
  var t286: f32;
  var t86: f32;
  var t224: f32;
  var t268: i32;
  var t50: f32;
  var t117: bool;
  var t188: f32;
  var t221: f32;
  var t162: f32;
  var t21: i32;
  var t302: f32;
  var t154: f32;
  var t248: f32;
  var t38: bool;
  var t274: f32;
  var t195: f32;
  var t151: f32;
  var t74: f32;
  var t233: f32;
  var t215: f32;
  var t170: f32;
  var t46: i32;
  var t207: f32;
  var t290: f32;
  var t90: f32;
  var t261: f32;
  var t122: i32;
  var t288: f32;
  var t183: f32;
  var t266: i32;
  var t125: bool;
  var t66: f32;
  var t200: f32;
  var t227: f32;
  var t27: bool;
  var t142: i32;
  var t219: f32;
  var t130: i32;
  var t276: f32;
  var t197: f32;
  var t179: f32;
  var t135: i32;
  var t213: f32;
  var t176: f32;
  var t13: bool;
  var t40: i32;
  var t35: i32;
                      
    
    t5 = f_gpu32_local_id_x(); 
    t8 = f_gpu32_local_id_x(); t9 = (i32(16) == i32(0)); 
    t12 = (t8 == i32(-2147483648)); t13 = (i32(16) == i32(-1)); t14 = (t12 & t13); 
    t17 = (t8 / i32(16)); t18 = (i32(4) * t17); 
    t21 = f_gpu32_local_id_x(); t22 = (i32(16) == i32(0)); 
    t25 = (t21 == i32(-2147483648)); t26 = (i32(16) == i32(-1)); t27 = (t25 & t26); 
    t30 = (t21 % i32(16)); t31 = (i32(4) * t30); 
    t34 = i32(fibw_wid.x); t35 = i32(fibw_wid.y); t36 = i32(fibw_wid.z); t37 = false; t38 = true; t39 = select(t36, t35, t38); t40 = select(t39, t34, t37);  t42 = bitcast<i32>(bitcast<u32>(t40)); t43 = (i32(64) * t42); 
    t46 = f_gpu32_group_id_x(); t47 = (i32(64) * t46); 
    
     t50 = f32(0.0);
    
    
     t54 = f32(0.0);
    
    
     t58 = f32(0.0);
    
    
     t62 = f32(0.0);
    
    
     t66 = f32(0.0);
    
    
     t70 = f32(0.0);
    
    
     t74 = f32(0.0);
    
    
     t78 = f32(0.0);
    
    
     t82 = f32(0.0);
    
    
     t86 = f32(0.0);
    
    
     t90 = f32(0.0);
    
    
     t94 = f32(0.0);
    
    
     t98 = f32(0.0);
    
    
     t102 = f32(0.0);
    
    
     t106 = f32(0.0);
    
    
     t110 = f32(0.0);
    
    t114 = i32(0);
    loop {
    t116 = t114; t117 = (t116 < p3); if t117 { 
    t121 = t114; f_load_slices(Ptr(16u, 0u), Ptr(17u, 0u), p0, p1, p3, t5, t43, t47, t121);
    workgroupBarrier();
    t122 = i32(0);
    loop {
    t124 = t122; t125 = (t124 < i32(16)); if t125 { 
    t129 = t122; t130 = (t129 * i32(64)); t131 = (t130 + t18); 
    t134 = t122; t135 = (t134 * i32(64)); t136 = (t135 + t31); 
    t139 = f_gpu32_f32_at(Ptr(16u, 0u), t131); 
    t142 = (t131 + i32(1)); t143 = f_gpu32_f32_at(Ptr(16u, 0u), t142); 
    t146 = (t131 + i32(2)); t147 = f_gpu32_f32_at(Ptr(16u, 0u), t146); 
    t150 = (t131 + i32(3)); t151 = f_gpu32_f32_at(Ptr(16u, 0u), t150); 
    t154 = f_gpu32_f32_at(Ptr(17u, 0u), t136); 
    t157 = (t136 + i32(1)); t158 = f_gpu32_f32_at(Ptr(17u, 0u), t157); 
    t161 = (t136 + i32(2)); t162 = f_gpu32_f32_at(Ptr(17u, 0u), t161); 
    t165 = (t136 + i32(3)); t166 = f_gpu32_f32_at(Ptr(17u, 0u), t165); 
     t170 = t50; t171 = fma(t139, t154, t170);  t173 = t50;  t50 = t171;
     t176 = t54; t177 = fma(t139, t158, t176);  t179 = t54;  t54 = t177;
     t182 = t58; t183 = fma(t139, t162, t182);  t185 = t58;  t58 = t183;
     t188 = t62; t189 = fma(t139, t166, t188);  t191 = t62;  t62 = t189;
     t194 = t66; t195 = fma(t143, t154, t194);  t197 = t66;  t66 = t195;
     t200 = t70; t201 = fma(t143, t158, t200);  t203 = t70;  t70 = t201;
     t206 = t74; t207 = fma(t143, t162, t206);  t209 = t74;  t74 = t207;
     t212 = t78; t213 = fma(t143, t166, t212);  t215 = t78;  t78 = t213;
     t218 = t82; t219 = fma(t147, t154, t218);  t221 = t82;  t82 = t219;
     t224 = t86; t225 = fma(t147, t158, t224);  t227 = t86;  t86 = t225;
     t230 = t90; t231 = fma(t147, t162, t230);  t233 = t90;  t90 = t231;
     t236 = t94; t237 = fma(t147, t166, t236);  t239 = t94;  t94 = t237;
     t242 = t98; t243 = fma(t151, t154, t242);  t245 = t98;  t98 = t243;
     t248 = t102; t249 = fma(t151, t158, t248);  t251 = t102;  t102 = t249;
     t254 = t106; t255 = fma(t151, t162, t254);  t257 = t106;  t106 = t255;
     t260 = t110; t261 = fma(t151, t166, t260);  t263 = t110;  t110 = t261;
    t265 = t122; t266 = (t265 + i32(1)); t122 = t266;
    continue;










 } else { break; }
}

    
    workgroupBarrier();
    t267 = t114; t268 = (t267 + i32(16)); t114 = t268;
    continue;


 } else { break; }
}

    
    t269 = (t43 + t18); t270 = (t47 + t31);  t272 = t50;  t274 = t54;  t276 = t58;  t278 = t62;  t280 = t66;  t282 = t70;  t284 = t74;  t286 = t78;  t288 = t82;  t290 = t86;  t292 = t90;  t294 = t94;  t296 = t98;  t298 = t102;  t300 = t106;  t302 = t110; f_store_tile(p2, p3, t269, t270, t272, t274, t276, t278, t280, t282, t284, t286, t288, t290, t292, t294, t296, t298, t300, t302);
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    return;





























}
@compute @workgroup_size(wg_x, wg_y, wg_z)
fn gemm_smem(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
  fibw_lid = lid; fibw_wid = wid; fibw_nwg = nwg;
  fibw_k_gemm_smem(Ptr(0u, 0u), Ptr(1u, 0u), Ptr(2u, 0u), bitcast<i32>(fibw_params[0].x));
}
fn fibw_k_assert_positive(a_p0: Ptr, a_p1: i32) {
  var p0: Ptr = a_p0;
  var p1: i32 = a_p1;
  var t8: f32;
  var t9: bool;
  var t4: bool;
  var t1: i32;
    t1 = f_gpu32_global_id_x(); 
    t4 = (t1 < p1); if t4 { 
    t8 = f_gpu32_f32_at(p0, t1); t9 = (t8 <= f32(0.0)); if t9 { 
    fib_trap(Ptr(0u, 0u)); if (fibw_trapped) { return; }
    return;
 } else {  }
    
    


 } else { 
    
 }
    return;


}
@compute @workgroup_size(wg_x, wg_y, wg_z)
fn assert_positive(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
  fibw_lid = lid; fibw_wid = wid; fibw_nwg = nwg;
  fibw_k_assert_positive(Ptr(0u, 0u), bitcast<i32>(fibw_params[0].x));
}
fn f_gpu32_global_id_x() -> i32 {
  var t5: bool;
  var t2: i32;
  var t22: i32;
  var t11: i32;
  var t14: i32;
  var t20: bool;
  var t18: i32;
  var t12: bool;
  var t9: i32;
  var t6: i32;
  var t3: i32;
  var t23: i32;
  var t10: i32;
  var t17: i32;
  var t4: bool;
  var t1: i32;
  var t21: bool;
  var t15: i32;
  var t7: i32;
  var t27: i32;
  var t19: i32;
  var t13: bool;
    t1 = i32(fibw_wid.x); t2 = i32(fibw_wid.y); t3 = i32(fibw_wid.z); t4 = true; t5 = false; t6 = select(t3, t2, t5); t7 = select(t6, t1, t4);  t9 = i32(wg_x); t10 = i32(wg_y); t11 = i32(wg_z); t12 = true; t13 = false; t14 = select(t11, t10, t13); t15 = select(t14, t9, t12);  t17 = i32(fibw_lid.x); t18 = i32(fibw_lid.y); t19 = i32(fibw_lid.z); t20 = true; t21 = false; t22 = select(t19, t18, t21); t23 = select(t22, t17, t20);    t27 = bitcast<i32>(((bitcast<u32>(t7) * bitcast<u32>(t15)) + bitcast<u32>(t23))); return t27;
}
fn f_gpu32_f32_at(a_p0: Ptr, a_p1: i32) -> f32 {
  var p0: Ptr = a_p0;
  var p1: i32 = a_p1;
  var t3: Ptr;
  var t4: f32;
  var t1: i32;
    t1 = (i32(4) * p1);  t3 = Ptr(p0.buf, p0.off + bitcast<u32>(t1)); t4 = fibw_ld_f32(t3); return t4;
}
fn f_gpu32_f32_set_(a_p0: Ptr, a_p1: i32, a_p2: f32) {
  var p0: Ptr = a_p0;
  var p1: i32 = a_p1;
  var p2: f32 = a_p2;
  var t3: Ptr;
  var t1: i32;
    t1 = (i32(4) * p1);  t3 = Ptr(p0.buf, p0.off + bitcast<u32>(t1)); fibw_st_f32(t3, p2);
    return;
}
fn f_gpu32_i32_at(a_p0: Ptr, a_p1: i32) -> i32 {
  var p0: Ptr = a_p0;
  var p1: i32 = a_p1;
  var t3: Ptr;
  var t4: i32;
  var t1: i32;
    t1 = (i32(4) * p1);  t3 = Ptr(p0.buf, p0.off + bitcast<u32>(t1)); t4 = fibw_ld_i32(t3); return t4;
}
fn f_gpu32_i32_set_(a_p0: Ptr, a_p1: i32, a_p2: i32) {
  var p0: Ptr = a_p0;
  var p1: i32 = a_p1;
  var p2: i32 = a_p2;
  var t3: Ptr;
  var t1: i32;
    t1 = (i32(4) * p1);  t3 = Ptr(p0.buf, p0.off + bitcast<u32>(t1)); fibw_st_i32(t3, p2);
    return;
}
fn f_gpu32_global_id_y() -> i32 {
  var t5: bool;
  var t2: i32;
  var t22: i32;
  var t11: i32;
  var t14: i32;
  var t20: bool;
  var t18: i32;
  var t12: bool;
  var t9: i32;
  var t6: i32;
  var t3: i32;
  var t23: i32;
  var t10: i32;
  var t17: i32;
  var t4: bool;
  var t1: i32;
  var t21: bool;
  var t15: i32;
  var t7: i32;
  var t27: i32;
  var t19: i32;
  var t13: bool;
    t1 = i32(fibw_wid.x); t2 = i32(fibw_wid.y); t3 = i32(fibw_wid.z); t4 = false; t5 = true; t6 = select(t3, t2, t5); t7 = select(t6, t1, t4);  t9 = i32(wg_x); t10 = i32(wg_y); t11 = i32(wg_z); t12 = false; t13 = true; t14 = select(t11, t10, t13); t15 = select(t14, t9, t12);  t17 = i32(fibw_lid.x); t18 = i32(fibw_lid.y); t19 = i32(fibw_lid.z); t20 = false; t21 = true; t22 = select(t19, t18, t21); t23 = select(t22, t17, t20);    t27 = bitcast<i32>(((bitcast<u32>(t7) * bitcast<u32>(t15)) + bitcast<u32>(t23))); return t27;
}
fn f_gpu32_local_id_x() -> i32 {
  var t5: bool;
  var t2: i32;
  var t9: i32;
  var t6: i32;
  var t3: i32;
  var t4: bool;
  var t1: i32;
  var t7: i32;
    t1 = i32(fibw_lid.x); t2 = i32(fibw_lid.y); t3 = i32(fibw_lid.z); t4 = true; t5 = false; t6 = select(t3, t2, t5); t7 = select(t6, t1, t4);  t9 = bitcast<i32>(bitcast<u32>(t7)); return t9;
}
fn f_gpu32_group_id_x() -> i32 {
  var t5: bool;
  var t2: i32;
  var t9: i32;
  var t6: i32;
  var t3: i32;
  var t4: bool;
  var t1: i32;
  var t7: i32;
    t1 = i32(fibw_wid.x); t2 = i32(fibw_wid.y); t3 = i32(fibw_wid.z); t4 = true; t5 = false; t6 = select(t3, t2, t5); t7 = select(t6, t1, t4);  t9 = bitcast<i32>(bitcast<u32>(t7)); return t9;
}
fn f_load_slices(a_p0: Ptr, a_p1: Ptr, a_p2: Ptr, a_p3: Ptr, a_p4: i32, a_p5: i32, a_p6: i32, a_p7: i32, a_p8: i32) {
  var p0: Ptr = a_p0;
  var p1: Ptr = a_p1;
  var p2: Ptr = a_p2;
  var p3: Ptr = a_p3;
  var p4: i32 = a_p4;
  var p5: i32 = a_p5;
  var p6: i32 = a_p6;
  var p7: i32 = a_p7;
  var p8: i32 = a_p8;
  var t45: bool;
  var t59: i32;
  var t8: i32;
  var t64: i32;
  var t25: bool;
  var t69: f32;
  var t53: i32;
  var t70: i32;
  var t49: bool;
  var t37: bool;
  var t32: i32;
  var t14: bool;
  var t62: f32;
  var t67: i32;
  var t20: i32;
  var t41: i32;
  var t12: bool;
  var t60: i32;
  var t29: i32;
  var t65: i32;
  var t58: i32;
  var t9: bool;
  var t26: bool;
  var t57: i32;
  var t3: i32;
  var t36: bool;
  var t17: i32;
  var t44: i32;
  var t63: i32;
  var t68: i32;
  var t4: bool;
  var t24: bool;
  var t50: bool;
  var t1: i32;
  var t21: bool;
  var t71: i32;
  var t48: bool;
  var t38: bool;
  var t33: bool;
  var t61: i32;
  var t66: i32;
  var t56: i32;
  var t13: bool;
     t1 = p5;
    loop {
    t3 = t1; t4 = (t3 < i32(1024)); if t4 { 
    t8 = t1; t9 = (i32(16) == i32(0)); 
    t12 = (t8 == i32(-2147483648)); t13 = (i32(16) == i32(-1)); t14 = (t12 & t13); 
    t17 = (t8 / i32(16)); 
    t20 = t1; t21 = (i32(16) == i32(0)); 
    t24 = (t20 == i32(-2147483648)); t25 = (i32(16) == i32(-1)); t26 = (t24 & t25); 
    t29 = (t20 % i32(16)); 
    t32 = t1; t33 = (i32(64) == i32(0)); 
    t36 = (t32 == i32(-2147483648)); t37 = (i32(64) == i32(-1)); t38 = (t36 & t37); 
    t41 = (t32 / i32(64)); 
    t44 = t1; t45 = (i32(64) == i32(0)); 
    t48 = (t44 == i32(-2147483648)); t49 = (i32(64) == i32(-1)); t50 = (t48 & t49); 
    t53 = (t44 % i32(64)); 
    t56 = (t29 * i32(64)); t57 = (t56 + t17); t58 = (p6 + t17); t59 = (t58 * p4); t60 = (p8 + t29); t61 = (t59 + t60); t62 = f_gpu32_f32_at(p2, t61); f_gpu32_f32_set_(p0, t57, t62);
    t63 = (t41 * i32(64)); t64 = (t63 + t53); t65 = (p8 + t41); t66 = (t65 * p4); t67 = (p7 + t53); t68 = (t66 + t67); t69 = f_gpu32_f32_at(p3, t68); f_gpu32_f32_set_(p1, t64, t69);
    t70 = t1; t71 = (t70 + i32(256)); t1 = t71;
    continue;












 } else { break; }
}

    
    return;


}
fn f_store_tile(a_p0: Ptr, a_p1: i32, a_p2: i32, a_p3: i32, a_p4: f32, a_p5: f32, a_p6: f32, a_p7: f32, a_p8: f32, a_p9: f32, a_p10: f32, a_p11: f32, a_p12: f32, a_p13: f32, a_p14: f32, a_p15: f32, a_p16: f32, a_p17: f32, a_p18: f32, a_p19: f32) {
  var p0: Ptr = a_p0;
  var p1: i32 = a_p1;
  var p2: i32 = a_p2;
  var p3: i32 = a_p3;
  var p4: f32 = a_p4;
  var p5: f32 = a_p5;
  var p6: f32 = a_p6;
  var p7: f32 = a_p7;
  var p8: f32 = a_p8;
  var p9: f32 = a_p9;
  var p10: f32 = a_p10;
  var p11: f32 = a_p11;
  var p12: f32 = a_p12;
  var p13: f32 = a_p13;
  var p14: f32 = a_p14;
  var p15: f32 = a_p15;
  var p16: f32 = a_p16;
  var p17: f32 = a_p17;
  var p18: f32 = a_p18;
  var p19: f32 = a_p19;
  var t16: i32;
  var t8: i32;
  var t5: i32;
  var t25: i32;
  var t2: i32;
  var t22: i32;
  var t11: i32;
  var t14: i32;
  var t20: i32;
  var t18: i32;
  var t23: i32;
  var t17: i32;
  var t24: i32;
  var t1: i32;
  var t21: i32;
  var t15: i32;
  var t19: i32;
    t1 = (p2 * p1); t2 = (t1 + p3); 
    t5 = (t2 + p1); 
    t8 = (t5 + p1); 
    t11 = (t8 + p1); 
    f_gpu32_f32_set_(p0, t2, p4);
    t14 = (t2 + i32(1)); f_gpu32_f32_set_(p0, t14, p5);
    t15 = (t2 + i32(2)); f_gpu32_f32_set_(p0, t15, p6);
    t16 = (t2 + i32(3)); f_gpu32_f32_set_(p0, t16, p7);
    f_gpu32_f32_set_(p0, t5, p8);
    t17 = (t5 + i32(1)); f_gpu32_f32_set_(p0, t17, p9);
    t18 = (t5 + i32(2)); f_gpu32_f32_set_(p0, t18, p10);
    t19 = (t5 + i32(3)); f_gpu32_f32_set_(p0, t19, p11);
    f_gpu32_f32_set_(p0, t8, p12);
    t20 = (t8 + i32(1)); f_gpu32_f32_set_(p0, t20, p13);
    t21 = (t8 + i32(2)); f_gpu32_f32_set_(p0, t21, p14);
    t22 = (t8 + i32(3)); f_gpu32_f32_set_(p0, t22, p15);
    f_gpu32_f32_set_(p0, t11, p16);
    t23 = (t11 + i32(1)); f_gpu32_f32_set_(p0, t23, p17);
    t24 = (t11 + i32(2)); f_gpu32_f32_set_(p0, t24, p18);
    t25 = (t11 + i32(3)); f_gpu32_f32_set_(p0, t25, p19); return;




}
