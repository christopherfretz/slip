#include "side.h"
// §8 PtrBase `Decl`: the address of a template parameter object (`&V` for a class-type `V`) is its `Ref`.
template <auto V> struct A;
struct S { int i; };
template <S V> struct Bt { static constexpr const S* p = &V; };
#ifdef B
A<Bt<S{2}>::p>* x;
#else
A<Bt<S{1}>::p>* x;
#endif
