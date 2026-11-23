// std: c++20
#include "side.h"
// §5.3 DeclKind `UsingEnum`: added.
enum class E { e };
#ifdef B
using enum E;
#endif
