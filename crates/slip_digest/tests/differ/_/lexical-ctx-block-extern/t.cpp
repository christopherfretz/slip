#include "side.h"
// §5.7 ColdDeclData `lexical_ctx`: an `extern` variable first declared in a block, against one first declared at namespace scope.
#ifdef B
void h() { extern int v; }
#else
extern int v;
void h() { extern int v; }
#endif
