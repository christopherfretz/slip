// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, templated: an out-of-line member of a nested class of a partial specialisation (`spelling/using-enum`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T, class U> struct S;
template<class T> struct S<T, int> { struct N { void h(); }; };
template<class T> void S<T, int>::N::h() { using enum Q<sizeof(sizeof(f(T())))>::E; }
