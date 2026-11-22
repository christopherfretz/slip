#include "side.h"
// §7 ExprNode: `CXXTypeidExpr.aux-potentially-evaluated`: the fact differs.
namespace std { class type_info; }
#ifdef B
struct P { void h(); };
extern P &p;
const std::type_info &v = typeid(p);
#else
struct P { virtual void h(); };
extern P &p;
const std::type_info &v = typeid(p);
#endif
