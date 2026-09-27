/*
 * Disposition d'un programme en mémoire : même principe que le noyau
 * (un segment par type de droits), dans la moitié basse de l'espace.
 */

ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(5);  /* R-X */
    rodata PT_LOAD FLAGS(4);  /* R-- */
    data   PT_LOAD FLAGS(6);  /* RW- */
}

SECTIONS
{
    . = 0x400000;

    .text : ALIGN(4K) {
        *(.text .text.*)
    } :text

    . = ALIGN(4K);
    .rodata : {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(4K);
    .data : {
        *(.data .data.*)
        *(.got .got.*)
    } :data

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    } :data

    /* Le manifeste d'un pilote (macro `pilote!`) : gardé dans le fichier,
     * jamais chargé en mémoire (INFO). */
    .aiwos.pilote 0 (INFO) : {
        KEEP(*(.aiwos.pilote))
    }

    /DISCARD/ : {
        *(.eh_frame*)
        *(.note .note.*)
        *(.comment)
    }
}
