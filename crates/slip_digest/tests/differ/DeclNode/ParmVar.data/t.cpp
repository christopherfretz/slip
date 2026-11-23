#include "side.h"
// §5.3 DeclNode `ParmVar.data`: the default argument.
#ifdef B
void f(int = 2);
#else
void f(int = 1);
#endif
