// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, templated: a requires-expression in a function template's requires-clause (`spelling/pseudo-destructor-qualifier`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> requires requires { p->Q<sizeof(sizeof(f(T())))>::I::~I(); } void g();
