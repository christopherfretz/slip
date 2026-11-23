// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, templated: a member template of an explicit specialisation (its own pattern; `spelling/using-enum`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S;
template<> struct S<int> { template<class T> struct M { void h() { using enum Q<sizeof(sizeof(f(T())))>::E; } }; };
