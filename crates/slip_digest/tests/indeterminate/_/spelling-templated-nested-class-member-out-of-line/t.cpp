// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, the templated declaration (`own_spelling::templated`): a member of a nested class of a class template, defined out of line, holding a qualifier whose operand holds the found set of `f`; Kiln keeps no written qualifier there, so the unit is indeterminate (`spelling/using-enum`) rather than equal.
template<int N> struct Q { using I = int; enum E { e1 }; };
int *p;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { struct N { void h(); }; };
template<class T> void S<T>::N::h() { using enum Q<sizeof(sizeof(f(T())))>::E; }
