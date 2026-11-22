#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDeleteExpr.bits-array-as-written`: a pointer to an array is deleted as an array either way (Kiln warns): only `delete[]` written differs.
int (*pa)[2];
#ifdef B
int v = (delete pa, 0);
#else
int v = (delete[] pa, 0);
#endif
