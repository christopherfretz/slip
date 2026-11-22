#include "side.h"
// std: c++26
// WP-028 review, digest §7: a 30-level chain whose levels each hold two pointers to the level below; the leaf value differs.
namespace std::meta {
using info = decltype(^^int);
template <class T> consteval T extract(info r);
template <class R> consteval info __kiln_reflect_constant_array(R&& r);
}
template <auto V> struct A;
#ifdef B
constexpr int a0[] = {2, 0};
#else
constexpr int a0[] = {1, 0};
#endif
constexpr const int* p0 = std::meta::extract<const int*>(std::meta::__kiln_reflect_constant_array(a0));
constexpr const void* a1[] = {p0, p0};
constexpr const void* const* p1 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a1));
constexpr const void* a2[] = {p1, p1};
constexpr const void* const* p2 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a2));
constexpr const void* a3[] = {p2, p2};
constexpr const void* const* p3 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a3));
constexpr const void* a4[] = {p3, p3};
constexpr const void* const* p4 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a4));
constexpr const void* a5[] = {p4, p4};
constexpr const void* const* p5 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a5));
constexpr const void* a6[] = {p5, p5};
constexpr const void* const* p6 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a6));
constexpr const void* a7[] = {p6, p6};
constexpr const void* const* p7 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a7));
constexpr const void* a8[] = {p7, p7};
constexpr const void* const* p8 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a8));
constexpr const void* a9[] = {p8, p8};
constexpr const void* const* p9 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a9));
constexpr const void* a10[] = {p9, p9};
constexpr const void* const* p10 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a10));
constexpr const void* a11[] = {p10, p10};
constexpr const void* const* p11 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a11));
constexpr const void* a12[] = {p11, p11};
constexpr const void* const* p12 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a12));
constexpr const void* a13[] = {p12, p12};
constexpr const void* const* p13 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a13));
constexpr const void* a14[] = {p13, p13};
constexpr const void* const* p14 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a14));
constexpr const void* a15[] = {p14, p14};
constexpr const void* const* p15 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a15));
constexpr const void* a16[] = {p15, p15};
constexpr const void* const* p16 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a16));
constexpr const void* a17[] = {p16, p16};
constexpr const void* const* p17 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a17));
constexpr const void* a18[] = {p17, p17};
constexpr const void* const* p18 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a18));
constexpr const void* a19[] = {p18, p18};
constexpr const void* const* p19 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a19));
constexpr const void* a20[] = {p19, p19};
constexpr const void* const* p20 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a20));
constexpr const void* a21[] = {p20, p20};
constexpr const void* const* p21 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a21));
constexpr const void* a22[] = {p21, p21};
constexpr const void* const* p22 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a22));
constexpr const void* a23[] = {p22, p22};
constexpr const void* const* p23 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a23));
constexpr const void* a24[] = {p23, p23};
constexpr const void* const* p24 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a24));
constexpr const void* a25[] = {p24, p24};
constexpr const void* const* p25 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a25));
constexpr const void* a26[] = {p25, p25};
constexpr const void* const* p26 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a26));
constexpr const void* a27[] = {p26, p26};
constexpr const void* const* p27 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a27));
constexpr const void* a28[] = {p27, p27};
constexpr const void* const* p28 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a28));
constexpr const void* a29[] = {p28, p28};
constexpr const void* const* p29 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a29));
constexpr const void* a30[] = {p29, p29};
constexpr const void* const* p30 = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a30));
A<p30>* x;
