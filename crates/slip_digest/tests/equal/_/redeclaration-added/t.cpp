#include "side.h"
// spec/digest.md §1 rule 2: redeclaration added does not matter.
#ifdef B
void f(int); void f(int);
#else
void f(int);
#endif
