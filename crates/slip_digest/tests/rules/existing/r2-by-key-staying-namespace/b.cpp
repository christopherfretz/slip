namespace acme { int x() { return 1; } int g() { return 2; } }
namespace legacy { int kept_fn() { return 3; } }
using namespace legacy;
