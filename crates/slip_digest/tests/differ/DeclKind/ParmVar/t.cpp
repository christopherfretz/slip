#include "side.h"
// §5.3 DeclKind `ParmVar`: through the function: a parameter with a default argument.
#ifdef B
void f(int = 1);
#else
void f(int);
#endif
