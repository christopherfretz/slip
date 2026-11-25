#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 fix round 1 (S4): one case per arm of `operands::scan` that the
// first delivery's suite did not reach (the review's probes and mutations
// R19, R20, R21, R22, R23, R24, R26), each with an operand whose binding
// moves (`lib::make` against `::make<lib::X>`).
template <class T> struct Wr { T m; };
template <int N> struct Q {
  static constexpr int v = N;
  struct In { static constexpr int w = N; };
  template <class U> struct Tn { struct inner { using t = int; }; };
};
// R19: a member pointer's class.
int Wr<decltype(make(x))>::* pm = nullptr;
// R20: a function type's parameters.
void (*fpp)(decltype(make(x))) = nullptr;
// R24: `typeof` of a type.
__typeof__(Wr<decltype(make(x))>) tot;
// R26: an array type trait's type.
unsigned long at = __array_rank(Wr<decltype(make(x))>[2]);
// R21: packs in written argument lists.
template <class... Ts> using VT = int;
VT<int, Q<sizeof(make(x))>> vp = 0;
template <class... Ts> struct Tup {};
template <class... Ts> struct TT { Tup<Ts...> m; };
TT<int, decltype(make(x))> tt;
// R22: a qualifier's identifier segment, whose prefix holds the operand.
template <class T> void f24(T, typename Q<sizeof(make(x))>::template Tn<T>::inner::t);
// R23: a qualifier applied to an array type through sugar.
const __typeof__(Wr<decltype(make(x))>[2]) qa = {};
template <class T> using Arr = T[2];
const Arr<Wr<decltype(make(x))>> qb = {};
