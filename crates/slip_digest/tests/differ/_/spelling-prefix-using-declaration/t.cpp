#include "side.h"
// §4.4 "Spellings", rule 11: a using-declaration whose qualifier holds an instantiation-dependent operand (`using Bq<..f(T())..>::value;` in a class template), `f` against `h`.
template<int N> struct Bq { static constexpr int value = 1; };
struct X {};
int f(int); int h(X);
#ifdef B
template<class T> struct D : Bq<8> { using Bq<sizeof(sizeof(h(T())))>::value; };
#else
template<class T> struct D : Bq<8> { using Bq<sizeof(sizeof(f(T())))>::value; };
#endif
