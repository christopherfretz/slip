// std: c++17
// target: aarch64-unknown-linux-gnu
// args: -fchar8_t
static_assert(static_cast<char>(-1) > 0, "char is unsigned on AArch64");
namespace feature {
int concept = 1;
char8_t c = 0;
}
