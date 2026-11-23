// WP-104 (digest §9), F3: block-scope static constexpr source_location
#include "srcloc.h"
void log(std::source_location);
namespace acme { void f() { static constexpr auto loc = std::source_location::current(); log(loc); } }
