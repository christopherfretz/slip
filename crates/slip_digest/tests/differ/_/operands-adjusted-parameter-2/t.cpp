#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct Q { static constexpr int v = N; };
template <class T> void p1(T, Q<sizeof(make(T()))> q[2]);
void u1() { p1(x, nullptr); }
template <class T> void p2(T, decltype(make(T())) q[sizeof(T)]);
void u2() { p2(x, nullptr); }
template <class T> void p3(T, decltype(make(T())) g(int));
void u3() { p3(x, nullptr); }
void p4(Q<sizeof(make(x))> a[3]);
void p5(decltype(make(x)) g());
template <class T> void p6(T, const decltype(make(T())) c[]);
void u6() { p6(x, nullptr); }
template <class T> void p7(T, Q<sizeof(make(T()))> g());
void u7() { p7(x, nullptr); }
