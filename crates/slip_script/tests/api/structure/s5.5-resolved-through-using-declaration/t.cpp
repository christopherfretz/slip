namespace a {
int f(int);
}
namespace b {
using a::f;
int g() { return f(1); }
}
