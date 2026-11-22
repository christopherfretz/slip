#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CompoundLiteralExpr.aux`: a namespace-scope array literal is a file-scope compound literal (unsupported).
int *v = (int[]){1, 2};
