namespace legacy {
int f();
struct Dep { static int s; struct Nested; void m(); };
}
namespace acme { int g(); }
