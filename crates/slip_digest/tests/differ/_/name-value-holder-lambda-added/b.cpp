// WP-115 item 2 (digest §9): a use in an init capture of a lambda at namespace scope is numbered among the uses of the variable whose initializer holds the lambda: a use added to another holder does not renumber l's
#include "srcloc.h"
constexpr int other = std::source_location::current().line();
constexpr auto l = [x = std::source_location::current(), y = std::source_location::current()] { return y; };
constexpr std::source_location G = l();
