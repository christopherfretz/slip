namespace acme { int a; }
namespace legacy { namespace { int b; } }
int u() { return acme::a + legacy::b; }
