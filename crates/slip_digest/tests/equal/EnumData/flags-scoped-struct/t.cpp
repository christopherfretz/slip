#include "side.h"
// §5.5 EnumData `flags-scoped-struct`: `enum struct` against `enum class`.
#ifdef B
enum struct E { a };
#else
enum class E { a };
#endif
