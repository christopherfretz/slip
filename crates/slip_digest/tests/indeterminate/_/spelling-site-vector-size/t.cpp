#include "side.h"
// §4.4 "Spellings", rule 5: `spelling/vector-size`, an adjusted array parameter of a function type that is not a declaration's own holds the canonical element type, whose operand has no spelling of its own.
int f(int);
template<class T> void g(T, void (*)(T __attribute__((vector_size(sizeof(f(T())) * 4))) [2]));
