#include "side.h"
// WP-035, digest-kinds §7 `_` `construct-qualified-name`: a constructor named unqualified inside `ns` or fully qualified.
namespace ns { struct S { S(int); }; }
#ifdef B
namespace ns { S v = ::ns::S(1); }
#else
namespace ns { S v = S(1); }
#endif
