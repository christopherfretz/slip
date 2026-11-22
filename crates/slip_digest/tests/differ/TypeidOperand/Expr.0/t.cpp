#include "side.h"
// §7 TypeidOperand: `Expr.0`: the fact differs.
namespace std { class type_info; }
int a = 1, b = 2, c = 3; long l;
#ifdef B
const std::type_info &v = typeid(b);
#else
const std::type_info &v = typeid(a);
#endif
