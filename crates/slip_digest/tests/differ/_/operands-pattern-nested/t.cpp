#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 fix round 1 (W11): an operand that is not
// instantiation-dependent inside an instantiation-dependent type of a
// pattern: an alias specialisation's argument, a dependent name's
// qualifier, a dependent expression's qualifier. No instantiation is
// made: only the pattern holds it.
template <class T> struct Wr { T m; };
template <class A1, class B1> using P2 = Wr<B1>;
template <int N> struct Q { template <class U> struct In { using t = int; static constexpr int v = 0; }; };
template <class T> void pf(T, P2<T, decltype(make(x))>);
template <class T> void f22(T, typename Q<sizeof(make(x))>::template In<T>::t);
template <class T> int f23(T) { return Q<sizeof(make(x))>::template In<T>::v; }
