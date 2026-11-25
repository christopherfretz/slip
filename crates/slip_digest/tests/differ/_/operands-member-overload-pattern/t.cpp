#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct Q { static constexpr int v = N; static int s; int g(); using t = int; Q(int); Q(); };
template <class T, unsigned long N> struct arr {};
struct C2 { template <int M> int m(int) { return M; } int m(long) { return 0; } };
template <class T> int p_um(C2 c, T t) { return c.m<sizeof(make(x))>(t); }
