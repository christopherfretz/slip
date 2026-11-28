namespace legacy { struct S { int v; }; }
namespace acme { int x(legacy::S s) { return s.v; } }
