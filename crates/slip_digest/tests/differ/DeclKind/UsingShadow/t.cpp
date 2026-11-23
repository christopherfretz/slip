#include "side.h"
// §5.3 DeclKind `UsingShadow`: through the using-declaration: a second shadow of `using n::f`.
#ifdef B
namespace n { void f(int); void f(long); }
#else
namespace n { void f(int); }
#endif
using n::f;
