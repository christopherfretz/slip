// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, the templated declaration (`own_spelling::templated`): an out-of-line member enumeration's underlying type, holding a qualifier whose operand holds the found set of `f`; Kiln keeps no written qualifier there, so the unit is indeterminate (`spelling/pseudo-destructor-qualifier`) rather than equal.
template<int N> struct Q { using I = int; enum E { e1 }; };
int *p;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { enum class E : int; };
template<class T> enum class S<T>::E : decltype(p->Q<sizeof(sizeof(f(T())))>::I::~I(), 0) { x };
