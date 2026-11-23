#include "dep.h"
namespace legacy { namespace detail { struct Mine {}; } }
int use() { return sizeof(legacy::detail::Mine) + sizeof(legacy::detail::Dep); }
