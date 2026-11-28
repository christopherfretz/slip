namespace acme { int x() { return 1; } }
namespace legacy { int g() { return 2; } int kept_fn() { return 3; } }
using namespace legacy;
