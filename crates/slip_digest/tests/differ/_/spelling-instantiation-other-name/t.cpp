#include "side.h"
// §4.4 "Spellings", rule 7: the sides name different functions in one canonical type `A<8>`: the profiles differ.
template<int N> struct A {};
int f(int);
int h(int);
#ifdef B
template<class T> auto g(T t) -> A<sizeof(sizeof(h(t)))>;
#else
template<class T> auto g(T t) -> A<sizeof(sizeof(f(t)))>;
#endif
