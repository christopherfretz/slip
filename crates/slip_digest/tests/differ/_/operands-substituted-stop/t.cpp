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
template <class T> using P = Wr<T>*;
template <class T, class U = decltype(make(T()))> using D = Wr<U>;
D<lib::X> d1;
template <class T> int fe(Wr<T>*);
int u1 = fe<decltype(make(x))>(nullptr);
template <class T> struct SS { P<T> m; };
SS<decltype(make(x))> ss;
template <class T> concept CC = requires { typename Wr<T>; };
template <CC T> int fc(T);
int u2 = fc<decltype(make(x))>(1);
template <class T> auto fr(T) -> P<T>;
int u3 = sizeof(fr<decltype(make(x))>(1));
template <class T, class U = P<decltype(make(T()))>> U fd(T);
int u4 = sizeof(fd(x));
template <class T> struct S1 { P<decltype(make(T()))> m; };
S1<lib::X> s1;
template <class T> struct S2 { using A = P<T>; A a; };
S2<decltype(make(x))> s2;
template <class T> P<T> f2(T);
int u5 = sizeof(f2<decltype(make(x))>(1));
template <class T> struct S3 { template <class U> using In = Wr<T>; In<int> i; };
S3<decltype(make(x))> s3;
