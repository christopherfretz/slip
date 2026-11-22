#include "side.h"
// §5.3 DeclNode `EnumConstant.data`: record EnumeratorData.
#ifdef B
enum E { a = 0 };
#else
enum E { a };
#endif
