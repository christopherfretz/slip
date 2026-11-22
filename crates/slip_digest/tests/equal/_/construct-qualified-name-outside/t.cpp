#include "side.h"
// WP-035, digest-kinds §7 `_` `construct-qualified-name-outside`: a constructor named qualified or fully qualified.
namespace ns { struct S { S(int); }; }
#ifdef B
ns::S v = ::ns::S(1);
#else
ns::S v = ns::S(1);
#endif
