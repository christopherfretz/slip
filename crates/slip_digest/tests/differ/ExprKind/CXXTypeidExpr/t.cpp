#include "side.h"
// §7 ExprKind: `CXXTypeidExpr`: the kind on side A only.
namespace std { class type_info; }
extern const std::type_info &r;
#ifdef B
const std::type_info &v = r;
#else
const std::type_info &v = typeid(int);
#endif
