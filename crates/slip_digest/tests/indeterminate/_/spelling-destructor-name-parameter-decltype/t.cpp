// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11: the same in a parameter's `decltype` (`spelling/destructor-name`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
template<int N> struct Bk { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, decltype(q->~X<sizeof(sizeof(f(t)))>()) * = nullptr);
