#include "side.h"
// §7.3 DeclRefExtra `qualifier`: the one name Kiln keeps a qualifier for,
// a variable template specialisation (kiln_sema templ/actions.rs:1187).
namespace ns { template<class T> T vt = T(); }
using namespace ns;
#ifdef B
int v = ns::vt<int>;
#else
int v = vt<int>;
#endif
