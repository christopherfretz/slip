#include "side.h"
// §5.3 DeclKind `Using`: added.
namespace n { void f(); }
#ifdef B
using n::f;
#endif
