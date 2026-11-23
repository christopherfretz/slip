#pragma redefine_extname f impl_f
extern "C" int g(int);

int caller() { return g(1); }
