// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, templated: a deduction guide's return type (`spelling/pseudo-destructor-qualifier`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { S(T); };
template<class T> S(T) -> S<decltype(p->Q<sizeof(sizeof(f(T())))>::I::~I(), T())>;
