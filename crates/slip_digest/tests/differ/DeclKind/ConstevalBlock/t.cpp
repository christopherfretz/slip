#include "side.h"
// std: c++26
// §5.6 DeclKind `ConstevalBlock`: against a `static_assert`.
#ifdef B
static_assert(true);
#else
consteval {}
#endif
