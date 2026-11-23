#include "dep.h"

namespace acme {
struct Mine {};
wrap(Mine) -> wrap<Mine>;
}

legacy::wrap<acme::Mine> user() { return legacy::wrap(acme::Mine()); }
