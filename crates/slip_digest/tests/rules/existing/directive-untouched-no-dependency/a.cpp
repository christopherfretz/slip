namespace acme { int x() { return 1; } }
namespace legacy { int g() { return 2; } }
using namespace acme;
int u() { return x() + legacy::g(); }
