#include "side.h"
// §8 UnionMember `at`: the first union active against the second.
template <auto V> struct A;
union W { int a; constexpr W() {} constexpr W(int x) : a(x) {} };
struct T { W u1; W u2; };
#ifdef B
A<T{W(), W(1)}>* x;
#else
A<T{W(1), W()}>* x;
#endif
