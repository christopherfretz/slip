#include "side.h"
// §10.1 AttrKind: `TYPE_NULLABLE`: the attribute on one side only.
#ifdef B
class _Nullable S {};
#else
class S {};
#endif
