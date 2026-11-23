#include "side.h"
// §4.4 "Spellings", rule 11: `using enum` in a body, naming an enumeration through a qualifier that holds an instantiation-dependent operand: Kiln keeps the enumeration only, so the unit is indeterminate (`spelling/using-enum`) on both sides rather than equal.
template<int N> struct Bq { enum E { E1 = 1 }; };
struct X {};
int f(int); int h(X);
#ifdef B
template<class T> int g(T t) { using enum Bq<sizeof(sizeof(h(t)))>::E; return E1; }
#else
template<class T> int g(T t) { using enum Bq<sizeof(sizeof(f(t)))>::E; return E1; }
#endif
