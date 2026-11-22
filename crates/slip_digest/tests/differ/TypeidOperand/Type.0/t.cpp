#include "side.h"
// §7 TypeidOperand: `Type.0`: the fact differs.
namespace std { class type_info; }
#ifdef B
const std::type_info &v = typeid(long);
#else
const std::type_info &v = typeid(int);
#endif
