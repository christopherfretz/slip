#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CompoundLiteralExpr.b`: the implicit variable of a file-scope compound literal (unsupported).
int *v = (int[]){1, 2};
