/* Memory region definitions for STM32F103Cx */

MEMORY {
    FLASH(rx)   : ORIGIN = 0x08000000, LENGTH = 63K 
    CFG(rw)     : ORIGIN = 0x0800fc00, LENGTH = 1K
    RAM(rwx)    : ORIGIN = 0x20000000, LENGTH = 20K
}

SECTIONS {
    __cfg_start = ORIGIN(CFG);
    __cfg_end = ORIGIN(CFG) + LENGTH(CFG);
}
