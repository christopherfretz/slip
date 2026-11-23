namespace { int a = 1; }
namespace n { int b = 2; }
int u() { return a + n::b; }
