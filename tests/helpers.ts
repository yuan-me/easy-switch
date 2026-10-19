import {expect,type Page} from '@playwright/test';

export async function setTheme(page:Page,theme:string){
 const previous=await page.locator('nav button[aria-current="page"]').innerText();
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('button',{name:theme,exact:true}).click();
 await expect(page.getByTestId('settings-save-state')).toHaveText('已自动保存');
 await page.getByRole('button',{name:previous,exact:true}).click();
}
