#include "side.h"
// §5.3 DeclNode `FileScopeAsm.data`: the string of a file-scope asm.
#ifdef B
asm("nop");
#else
asm("");
#endif
