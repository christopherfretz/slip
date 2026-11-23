namespace s { int f(); }
namespace __attribute__((visibility("default"))) s { int f(); }
int u() { return s::f(); }
