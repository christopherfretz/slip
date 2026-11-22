#include "side.h"
// §5.7 ColdDeclData `lexical_ctx`: a member defined in its class has none, wherever the class is.
#ifdef B
namespace N { struct X { void m() {} }; }
#else
namespace N {
struct X {

  void m() {} };
}
#endif
