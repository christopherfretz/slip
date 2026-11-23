#include "side.h"
// §5.3 DeclKind `Label`: through the function that holds it.
#ifdef B
void f() { l:; }
#else
void f() { }
#endif
