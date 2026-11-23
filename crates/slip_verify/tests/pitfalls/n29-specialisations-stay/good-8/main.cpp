#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
wrap(acme::Mine) -> wrap<acme::Mine>;
}

legacy::wrap<acme::Mine> user() { return legacy::wrap(acme::Mine()); }
