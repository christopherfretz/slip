#include "dep.h"
namespace acme { namespace detail { struct Mine {}; } }
int use() { return sizeof(acme::detail::Mine) + sizeof(legacy::detail::Dep); }
