// errors: yes
#include "side.h"
// §7.1 ExprDeps `0-error`.
int a;
int v = a + undeclared;
