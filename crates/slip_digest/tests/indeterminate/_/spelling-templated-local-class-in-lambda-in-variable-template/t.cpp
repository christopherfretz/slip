// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, templated: a local class inside a lambda in a variable template's initialiser (`spelling/using-enum`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
template<int N> struct Bk { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int v = [] { struct L { int k() { using enum Q<sizeof(sizeof(f(T())))>::E; return 0; } }; return 0; }();
