#ifdef POST
#define DECL_INIT extern "C" int gen_init();
#else
#define DECL_INIT extern "C" int gen_start();
#endif
