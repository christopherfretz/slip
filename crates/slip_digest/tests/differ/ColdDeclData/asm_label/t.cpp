#include "side.h"
// §5.7 ColdDeclData `asm_label`: an asm label, on the first declaration.
#ifdef B
extern int x asm("y");
#else
extern int x;
#endif
int x = 1;
