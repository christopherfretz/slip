#include "side.h"
// §4.4 "Spellings", rule 5: `spelling/bit-int`, Kiln holds a dependent `_BitInt` canonical, with the first spelling's width.
int f(int);
template<class T> void g(T, _BitInt(sizeof(f(T())) * 8));
