#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
decltype(make(x)) o_decltype;
__typeof__(make(x)) o_typeof;
char o_bound[sizeof(make(x))];
template <class T, unsigned long N> struct arr {};
arr<char, sizeof(make(x))> o_targ;
void o_noexcept() noexcept(noexcept(make(x)));
void (*o_fp)() noexcept(noexcept(make(x)));
struct E { explicit(sizeof(make(x)) == 4) E(int); };
typedef decltype(make(x)) o_typedef;
using o_alias = decltype(make(x));
struct F { int bf : sizeof(make(x)); };
struct F2 { decltype(make(x)) fld; };
enum En { en = sizeof(make(x)) };
alignas(sizeof(make(x))) int o_align;
void o_param(int p[sizeof(make(x))]);
void o_param2(decltype(make(x)) q);
void o_dflt(int q = sizeof(make(x)));
auto o_trailing() -> decltype(make(x));
struct D : arr<char, sizeof(make(x))> {};
template <int N> int vt = N;
int o_vt = vt<sizeof(make(x))>;
template <int N> int ft() { return N; }
int o_ft = ft<sizeof(make(x))>();
static_assert(sizeof(make(x)) == 4);
auto o_lambda = [](decltype(make(x)) p) { return p; };
void o_local() { decltype(make(x)) l1; }
void o_local2() { char l2[sizeof(make(x))]; }
void o_local3() { typedef decltype(make(x)) LT; }
int o_cast = static_cast<decltype(make(x))>(0);
int o_sizeof_type = sizeof(decltype(make(x)));
struct Fr { friend decltype(make(x)) fr_f(); };
template <class T = decltype(make(x))> struct o_tdflt {};
template <int N = sizeof(make(x))> struct o_ndflt {};
template <class T> concept o_C = sizeof(make(x)) == 4;
template <class T> requires (sizeof(make(x)) == 4) struct o_req {};
template <> int vt<sizeof(make(x)) + 100> = 7;
template <int N> struct o_es {};
template <> struct o_es<sizeof(make(x))> {};
template struct arr<int, sizeof(make(x))>;
int o_case(int i) { switch (i) { case sizeof(make(x)): return 1; } return 0; }
[[deprecated]] int o_attr [[gnu::aligned(sizeof(make(x)))]];
