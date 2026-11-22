#include "side.h"
// §8 Reloc, PtrBase `Decl`: a class holding a pointer to a variable of class type and an array.
template <auto V> struct A;
struct P { int i; };
constexpr P p1{1}, p2{1};
struct Q { const P* p; int a[1]; };
#ifdef B
A<Q{&p2, {1}}>* x;
#else
A<Q{&p1, {1}}>* x;
#endif
