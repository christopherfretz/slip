#define DECLARE(name) void name(int);
#define DECLARE_F void f(int);
namespace n {
void f(int);
DECLARE(f)
DECLARE_F
}
