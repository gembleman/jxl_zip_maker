a program that converts PNG and JPG files to JXL and packs them into a ZIP file.

# how does this program benefit you?
..compare to using other methods like XnView converter, online JXL converter, etc.

1. using multiprocessing. so even faster.
2. you can customize your own settings for each image file format.
3. can make zip file.
4. It's free from dependency. When the jxl encoder(cjxl.exe) is updated, just replace it.
5. written in Rust(?)

# how to use?
just run and drop work folder or give argument before run
```cmd
jxl_zip_maker.exe {work_folder_path}
```
or
```cmd
C:>jxl_zip_maker.exe
Failed to read cjxl_args.ini
cjxl_args:
            delete_folder=false
            delete_source_image=false
            make_zip=true
            dont_use_trashcan_just_delete=false
            png_args=["--distance=0", "--effort=7"]
            jpg_args=["--distance=0", "--effort=9", "--lossless_jpeg=1"]
current cjxl.exe location: "C:\\cjxl.exe"
Drag&Drop folder to convert jxl and changed zip you want:
{work_folder_path}
```

# what is cjxl_args.ini?
```txt
//default setting
delete_folder=false
delete_source_image=false
make_zip=true
dont_use_trashcan_just_delete=false
png_args=[--distance=0,--effort=7]
jpg_args=[--distance=0,--effort=9,--lossless_jpeg=1]
```
1. delete_folder : true is delete, false is not.
2. delete_source_image : true is delete source image, false is not.
3. make_zip : true is make zip file, false is not.
4. dont_use_trashcan_just_delete : true is !!JUST DLETE IMAGE FILE!! so set it up when you expect to run out of disk space. false is image file throw trash can.
5. png_args : customize your own settings, just don't include spaces in your settings.
6. jpg_args : same.

# note
1. this program runs multiple jxl encoders, so the more cores there are in cpu, the more efficient.  
2. it recursively scans the working folder, so it doesn't matter how deep the image is in the working folder. exmple) workfolder/a_folder/b_foler/a.jpg is also convert.  
3. the zip file compression method is Stored. not LZMA, Deflare, std-z etc. because jxl file is already compressed. so meanless.  
4. If any of the files in a folder are not successfully converted, the folder is not deleted and no archive is created.

# why did I make it?
i was inspired to create this program because I wanted to optimize hundreds of thousands of photos stored on my hard disk.  
hard disks have poor random access performance compared to SSDs, so when you have hundreds of thousands of small files, it's very slow to index and move them around.  
so the idea was to zip those small files together. And also to reduce the amount of storage.  
to me, the JXL format seemed like the best choice for storing lossless images.  
and... here's the result.  
