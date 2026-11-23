// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11: a destructor named by member access with arguments in a trailing return type's `decltype` (rule 1's reading, `own_spelling::node`; `spelling/destructor-name`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
template<int N> struct Bk { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(q->~X<sizeof(sizeof(f(t)))>());
