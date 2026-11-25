#include "dep.h"

template <class T> int make(T);
template <class T, class U> concept Same = __is_same(T, U);
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };

template <class A1, class B1> using P2 = Wr<B1>;
namespace acme {
template <class T> void pf(T, P2<T, decltype(make(0))>);
template <int N> struct Q { template <class U> struct In { using t = int; }; };
template <class T> void f22(T, typename Q<sizeof(make(0))>::template In<T>::t);
}
