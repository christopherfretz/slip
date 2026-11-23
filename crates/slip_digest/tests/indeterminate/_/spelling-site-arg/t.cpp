#include "side.h"
// §4.4 "Spellings", rule 5: `spelling/arg`, an adjusted array parameter of a function type that is not a declaration's own holds the canonical element type, whose operand has no spelling of its own.
int f(int);
template<int N> struct S {};
template<class T> void g(T, void (*)(S<sizeof(f(T()))> [2]));
