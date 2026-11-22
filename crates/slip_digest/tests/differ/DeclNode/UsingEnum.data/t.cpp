#include "side.h"
// §5.3 DeclNode `UsingEnum.data`.
enum class E { e }; enum class F { f };
#ifdef B
using enum F;
#else
using enum E;
#endif
