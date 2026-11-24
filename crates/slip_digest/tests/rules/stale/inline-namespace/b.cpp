namespace acme { inline namespace v1 { int a; } }
namespace legacy { inline namespace v1 { int b; } }
int u() { return acme::a + legacy::b; }
