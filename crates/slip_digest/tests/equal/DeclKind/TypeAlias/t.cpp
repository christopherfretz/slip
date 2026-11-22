#include "side.h"
// §5.3 DeclKind `TypeAlias`: written as Typedef.
#ifdef B
using X = int;
#else
typedef int X;
#endif
