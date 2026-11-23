// WP-104 (digest §9), known cost G3: one ordinal counter for every non-function context of the unit
#include "srcloc.h"
constexpr unsigned g1 = 0;
constexpr std::source_location g2 = std::source_location::current();
